<?php
interface CustomThrowable {}
class CustomException extends Exception implements CustomThrowable {}

/** @psalm-suppress InvalidCatch */
try {
    throw new CustomException("Bad");
} catch (CustomThrowable $e) {
    throw $e;
}
