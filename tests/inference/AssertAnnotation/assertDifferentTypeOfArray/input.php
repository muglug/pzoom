<?php
/**
 * @psalm-assert list{string, string} $value
 * @param mixed $value
 */
function isStringTuple($value): void {
    if (!is_array($value)
        || !isset($value[0])
        || !isset($value[1])
        || !is_string($value[0])
        || !is_string($value[1])
    ) {
        throw new \Exception("bad");
    }
}

$s = "Hello World!";

$parts = explode(":", $s, 2);

isStringTuple($parts);

echo $parts[0];
echo $parts[1];
