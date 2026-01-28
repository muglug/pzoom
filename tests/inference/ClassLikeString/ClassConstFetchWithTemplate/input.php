<?php
/**
 * @template T of object
 * @psalm-param T $obj
 * @return class-string<T>
 */
function a($obj) {
    $class = $obj::class;

    return $class;
}
