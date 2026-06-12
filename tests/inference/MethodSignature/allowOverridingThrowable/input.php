<?php
/**
 * @psalm-immutable
 */
interface MyException extends \Throwable
{
    /**
     * Informative comment
     */
    public function getMessage(): string;
    public function getCode();
    public function getFile(): string;
    public function getLine(): int;
    public function getTrace(): array;
    public function getPrevious(): ?\Throwable;
    public function getTraceAsString(): string;
}
