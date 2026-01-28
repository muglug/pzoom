<?php
/**
 * @param class-string $s
 */
function takesOpen(string $s): void {}

/**
 * @param class-string<Exception> $s
 */
function takesException(string $s): void {}

/**
 * @param class-string<Exception> $s
 */
function takesThrowable(string $s): void {}

takesOpen(InvalidArgumentException::class);
takesException(InvalidArgumentException::class);
takesThrowable(InvalidArgumentException::class);
