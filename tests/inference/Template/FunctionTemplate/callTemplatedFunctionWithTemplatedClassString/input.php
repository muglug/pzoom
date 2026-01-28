<?php
/**
 * @template Ta of object
 * @psalm-param Ta $obj
 * @return Ta
 */
function a(string $str, object $obj) {
    $class = get_class($obj);
    return deserialize_object($str, $class);
}

/**
 * @psalm-template Tb
 * @psalm-param class-string<Tb> $type
 * @psalm-return Tb
 * @psalm-suppress InvalidReturnType
 */
function deserialize_object(string $data, string $type) {}