package dev.marcal.chatvault.application.chat

import dev.marcal.chatvault.api.dto.input.FileTypeInputEnum
import dev.marcal.chatvault.api.dto.input.NewMessagePayloadInput
import dev.marcal.chatvault.api.dto.mapper.toNewMessageInput
import dev.marcal.chatvault.api.web.exception.ChatImporterException
import dev.marcal.chatvault.application.message.ChatMessageParserService
import dev.marcal.chatvault.application.message.MessageCreatorService
import dev.marcal.chatvault.domain.bucket.BucketService
import dev.marcal.chatvault.domain.model.BucketFile
import dev.marcal.chatvault.domain.model.ChatNamePatternMatcher
import dev.marcal.chatvault.domain.repository.ChatRepository
import org.slf4j.LoggerFactory
import org.springframework.stereotype.Service
import java.io.BufferedInputStream
import java.io.FileInputStream
import java.io.InputStream
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.time.LocalDateTime
import java.util.zip.ZipInputStream

@Service
class ChatImportService(
    private val chatMessageParser: ChatMessageParserService,
    private val messageCreator: MessageCreatorService,
    private val bucketService: BucketService,
    private val chatRepository: ChatRepository,
    private val chatCreator: ChatCreatorService,
) {
    private val logger = LoggerFactory.getLogger(this.javaClass)

    fun execute(
        chatId: Long,
        inputStream: InputStream,
        fileType: FileTypeInputEnum,
    ) {
        when (fileType) {
            FileTypeInputEnum.ZIP -> {
                iterateOverZip(chatId, inputStream)
            }

            FileTypeInputEnum.TEXT -> {
                createMessages(inputStream = inputStream, chatId = chatId)
            }

            else -> throw IllegalStateException("file type $fileType not supported")
        }
    }

    fun execute(
        chatName: String?,
        inputStream: InputStream,
        fileType: FileTypeInputEnum,
    ) {
        val effectiveName =
            chatName?.takeIf { it.isNotBlank() }
                ?: "todo imported at ${LocalDateTime.now()}"

        val chatBucketInfo = chatCreator.findOrCreateByName(effectiveName)

        execute(chatBucketInfo.chatId, inputStream, fileType)
    }

    private fun iterateOverZip(
        chatId: Long,
        inputStream: InputStream,
    ) {
        val chatBucketInfo =
            chatRepository.findChatBucketInfoByChatId(chatId = chatId)
                ?: throw ChatImporterException("Chat id $chatId was not found. File import failed.")
        val bucket = chatBucketInfo.bucket.withPath("/")

        ZipInputStream(BufferedInputStream(inputStream)).use { zipInputStream ->
            var entry = zipInputStream.nextEntry
            while (entry != null) {
                if (!entry.isDirectory) {
                    val fileName = entry.name
                    val tempFile = Files.createTempFile("chatvault-import-", ".entry")

                    try {
                        Files.copy(zipInputStream, tempFile, StandardCopyOption.REPLACE_EXISTING)

                        FileInputStream(tempFile.toFile()).use { fileStream ->
                            bucketService.save(BucketFile(stream = fileStream, fileName = fileName, address = bucket))
                        }

                        if (ChatNamePatternMatcher.matches(fileName)) {
                            FileInputStream(tempFile.toFile()).use { textStream ->
                                execute(
                                    chatId = chatId,
                                    inputStream = textStream,
                                    fileType = FileTypeInputEnum.TEXT,
                                )
                            }
                        }
                    } finally {
                        Files.deleteIfExists(tempFile)
                    }
                }

                zipInputStream.closeEntry()
                entry = zipInputStream.nextEntry
            }
        }
    }

    private fun createMessages(
        inputStream: InputStream,
        chatId: Long,
    ) {
        val messages =
            chatMessageParser.parseAndTransform(inputStream) { messageOutput ->
                messageOutput.toNewMessageInput(chatId = chatId)
            }

        val count = chatRepository.countChatMessages(chatId)

        messageCreator.execute(
            NewMessagePayloadInput(
                chatId = chatId,
                messages = messages,
            ),
        )

        val countUpdated = chatRepository.countChatMessages(chatId)

        val newMessages = countUpdated - count

        logger.info("imported $newMessages of ${messages.size} messages  to chatId=$chatId")
    }
}
