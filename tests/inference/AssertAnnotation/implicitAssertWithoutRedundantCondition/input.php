<?php
namespace Bar;

/**
 * @param mixed $data
 * @throws \Exception
 */
function assertIsLongString($data): void {
    if (!\is_string($data)) {
        throw new \Exception;
    }
    if (strlen($data) < 100) {
        throw new \Exception;
    }
}

/**
 * @throws \Exception
 */
function f(string $s): void {
    assertIsLongString($s);
}
