package dev.marcal.chatvault.infrastructure.bucket

import dev.marcal.chatvault.api.web.exception.AttachmentFinderException
import dev.marcal.chatvault.api.web.exception.AttachmentNotFoundException
import dev.marcal.chatvault.api.web.exception.BucketFileNotFoundException
import dev.marcal.chatvault.api.web.exception.BucketServiceException
import dev.marcal.chatvault.domain.bucket.BucketService
import dev.marcal.chatvault.domain.model.Bucket
import dev.marcal.chatvault.domain.model.BucketFile
import io.minio.BucketExistsArgs
import io.minio.GetObjectArgs
import io.minio.ListObjectsArgs
import io.minio.MakeBucketArgs
import io.minio.MinioClient
import io.minio.PutObjectArgs
import io.minio.RemoveObjectArgs
import io.minio.RemoveObjectsArgs
import io.minio.StatObjectArgs
import io.minio.errors.ErrorResponseException
import io.minio.messages.DeleteObject
import jakarta.annotation.PostConstruct
import org.slf4j.LoggerFactory
import org.springframework.beans.factory.annotation.Value
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty
import org.springframework.core.io.InputStreamResource
import org.springframework.core.io.Resource
import org.springframework.core.io.UrlResource
import org.springframework.stereotype.Service
import java.io.BufferedWriter
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.FileWriter
import java.io.IOException
import java.nio.file.Files
import java.nio.file.Paths
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

@Service
@ConditionalOnProperty(name = ["chatvault.bucket.provider"], havingValue = "s3")
class S3BucketServiceImpl(
    @Value("\${chatvault.bucket.import}") private val bucketImportPath: String,
    @Value("\${chatvault.bucket.export}") private val bucketExportPath: String,
    @Value("\${chatvault.bucket.s3.endpoint}") private val endpoint: String,
    @Value("\${chatvault.bucket.s3.access-key}") private val accessKey: String,
    @Value("\${chatvault.bucket.s3.secret-key}") private val secretKey: String,
    @Value("\${chatvault.bucket.s3.bucket}") private val bucketName: String,
) : BucketService {
    private val logger = LoggerFactory.getLogger(this.javaClass)
    private val client: MinioClient by lazy {
        MinioClient
            .builder()
            .endpoint(endpoint)
            .credentials(accessKey, secretKey)
            .build()
    }

    @PostConstruct
    fun init() {
        createDirIfNotExists(File(bucketImportPath))
        createDirIfNotExists(File(bucketExportPath))

        val exists =
            client.bucketExists(
                BucketExistsArgs
                    .builder()
                    .bucket(bucketName)
                    .build(),
            )

        if (!exists) {
            client.makeBucket(
                MakeBucketArgs
                    .builder()
                    .bucket(bucketName)
                    .build(),
            )
        }
    }

    override fun save(bucketFile: BucketFile) {
        val objectName = objectName(bucketFile)
        try {
            val bytes = bucketFile.bytes
            if (bytes != null) {
                client.putObject(
                    PutObjectArgs
                        .builder()
                        .bucket(bucketName)
                        .`object`(objectName)
                        .stream(bytes.inputStream(), bytes.size.toLong(), -1)
                        .build(),
                )
            } else {
                val inputStream = requireNotNull(bucketFile.stream)
                client.putObject(
                    PutObjectArgs
                        .builder()
                        .bucket(bucketName)
                        .`object`(objectName)
                        .stream(inputStream, -1, 10L * 1024L * 1024L)
                        .build(),
                )
            }

            logger.info("File saved to s3://{}/{}", bucketName, objectName)
        } catch (e: Exception) {
            throw BucketServiceException("failed to save ${bucketFile.fileName} to S3", e)
        }
    }

    override fun saveToImportDir(bucketFile: BucketFile) {
        saveToLocalBucket(bucketFile, bucketImportPath)
    }

    override fun saveTextToBucket(
        bucketFile: BucketFile,
        messages: Sequence<String>,
    ) {
        val tempFile = Files.createTempFile("chatvault-text-", ".txt")
        try {
            BufferedWriter(FileWriter(tempFile.toFile())).use { writer ->
                messages.forEach { messageLine ->
                    writer.write(messageLine)
                    writer.newLine()
                }
            }

            FileInputStream(tempFile.toFile()).use { stream ->
                save(BucketFile(stream = stream, fileName = bucketFile.fileName, address = bucketFile.address))
            }
        } catch (ex: Exception) {
            throw BucketFileNotFoundException("File to save ${bucketFile.fileName}. Unexpected S3 error", ex)
        } finally {
            Files.deleteIfExists(tempFile)
        }
    }

    override fun loadBucketAsZip(path: String): Resource {
        val normalizedPrefix = normalizePrefix(path)
        return zipObjects(filename = "${normalizedPrefix.trimEnd('/')}.zip", prefix = normalizedPrefix)
    }

    override fun loadBucketListAsZip(): Resource = zipObjects(filename = "chatvault.zip", prefix = "")

    override fun delete(bucketFile: BucketFile) {
        val prefix = normalizePrefix(bucketFile.address.path)
        val objects =
            client
                .listObjects(
                    ListObjectsArgs
                        .builder()
                        .bucket(bucketName)
                        .prefix(prefix)
                        .recursive(true)
                        .build(),
                ).map { DeleteObject(it.get().objectName()) }

        client.removeObjects(
            RemoveObjectsArgs
                .builder()
                .bucket(bucketName)
                .objects(objects)
                .build(),
        ).forEach { result -> result.get() }
    }

    override fun zipPendingImports(chatName: String?): Sequence<Resource> {
        try {
            return File(bucketImportPath)
                .getDirectoriesWithContentAndZipFiles()
                .asSequence()
                .filter { chatName == null || chatName == it.name }
                .map { chatGroupDir ->
                    if (chatGroupDir.name.endsWith(".zip")) {
                        UrlResource(chatGroupDir.toURI())
                    } else {
                        DirectoryZipper.zipAndDeleteSource(chatGroupDir)
                    }
                }
        } catch (e: Exception) {
            throw BucketServiceException(message = "Fail to zip pending imports", throwable = e)
        }
    }

    override fun deleteZipImported(filename: String) {
        val toDelete =
            BucketFile(
                fileName = filename,
                address = Bucket(path = "/"),
            ).file(root = bucketImportPath)
        toDelete.delete()
    }

    override fun loadFileAsResource(bucketFile: BucketFile): Resource {
        val objectName = objectName(bucketFile)
        return try {
            val stat =
                client.statObject(
                    StatObjectArgs
                        .builder()
                        .bucket(bucketName)
                        .`object`(objectName)
                        .build(),
                )
            val stream =
                client.getObject(
                    GetObjectArgs
                        .builder()
                        .bucket(bucketName)
                        .`object`(objectName)
                        .build(),
                )

            object : InputStreamResource(stream) {
                override fun getFilename(): String = bucketFile.fileName

                override fun contentLength(): Long = stat.size()
            }
        } catch (e: ErrorResponseException) {
            if (e.errorResponse().code() == "NoSuchKey") {
                throw AttachmentNotFoundException("file not found ${bucketFile.fileName}")
            }
            throw AttachmentFinderException("failed to load file ${bucketFile.fileName}", e)
        } catch (e: Exception) {
            throw AttachmentFinderException("failed to load file ${bucketFile.fileName}", e)
        }
    }

    private fun zipObjects(
        filename: String,
        prefix: String,
    ): Resource {
        val zipFile = File(bucketExportPath, filename)
        try {
            createDirIfNotExists(zipFile.parentFile)
            FileOutputStream(zipFile).use { fos ->
                ZipOutputStream(fos).use { zipOut ->
                    client
                        .listObjects(
                            ListObjectsArgs
                                .builder()
                                .bucket(bucketName)
                                .prefix(prefix)
                                .recursive(true)
                                .build(),
                        ).forEach { result ->
                            val item = result.get()
                            if (!item.isDir) {
                                val key = item.objectName()
                                val entryName = key.removePrefix(prefix).trimStart('/').ifBlank { key }
                                zipOut.putNextEntry(ZipEntry(entryName))
                                client
                                    .getObject(
                                        GetObjectArgs
                                            .builder()
                                            .bucket(bucketName)
                                            .`object`(key)
                                            .build(),
                                    ).use { input -> input.copyTo(zipOut) }
                                zipOut.closeEntry()
                            }
                        }
                }
            }

            return InputStreamResource(
                object : FileInputStream(zipFile) {
                    @Throws(IOException::class)
                    override fun close() {
                        super.close()
                        val isDeleted = zipFile.delete()
                        logger.info("export:'{}':{}", zipFile.name, if (isDeleted) "deleted" else "preserved")
                    }
                },
            )
        } catch (e: Exception) {
            throw BucketServiceException(message = "Fail to zip S3 bucket prefix $prefix", throwable = e)
        }
    }

    private fun objectName(bucketFile: BucketFile): String {
        val normalized =
            Paths
                .get(bucketFile.address.path, bucketFile.fileName)
                .normalize()
                .toString()
                .replace(File.separatorChar, '/')
                .trimStart('/')

        if (normalized.isBlank() || normalized == "." || normalized.startsWith("..") || "/.." in normalized) {
            throw BucketServiceException("bad S3 object path for ${bucketFile.fileName}", null)
        }

        return normalized
    }

    private fun normalizePrefix(path: String): String {
        val normalized =
            Paths
                .get(path)
                .normalize()
                .toString()
                .replace(File.separatorChar, '/')
                .trim('/')

        if (normalized.isBlank() || normalized == ".") {
            return ""
        }

        if (normalized.startsWith("..") || "/.." in normalized) {
            throw BucketServiceException("bad S3 prefix $path", null)
        }

        return "$normalized/"
    }

    private fun saveToLocalBucket(
        bucketFile: BucketFile,
        rootPath: String,
    ) {
        try {
            val file = bucketFile.file(rootPath).also { createDirIfNotExists(it.parentFile) }
            bucketFile.bytes?.let { bytes ->
                FileOutputStream(file).use { it.write(bytes) }
            } ?: requireNotNull(bucketFile.stream).use { input ->
                FileOutputStream(file).use { output -> input.copyTo(output) }
            }
        } catch (e: Exception) {
            throw BucketServiceException("failed to save ${bucketFile.fileName} to local import dir", e)
        }
    }

    private fun createDirIfNotExists(file: File) {
        file.takeIf { !it.exists() }?.also {
            if (!it.mkdirs()) {
                throw BucketServiceException("failed to create directory $file", null)
            }
        }
    }
}
