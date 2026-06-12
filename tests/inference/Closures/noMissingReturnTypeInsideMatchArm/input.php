<?php
function f(?string $mode): callable {
    $fn = match ($mode) {
        'code' => static fn(string $file, int $line) => 'code --goto ' . $file . ':' . $line,
        null => throw new \AssertionError("no ide"),
        default => throw new \AssertionError("unknown"),
    };
    return $fn;
}
