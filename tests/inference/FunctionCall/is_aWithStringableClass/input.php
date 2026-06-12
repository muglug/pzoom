<?php
/**
 * @psalm-var class-string<Throwable> $exceptionType
 */
if (\is_a(new Exception(), $exceptionType)) {}
