<?php
/**
 * @psalm-template T of object
 * @psalm-param T|class-string<T> $someType
 * @psalm-return T
 * @psalm-suppress MixedMethodCall
 */
function getObject($someType) {
    if (is_object($someType)) {
        return $someType;
    }

    return new $someType();
}

class C {
    function sayHello() : string {
        return "hi";
    }
}

getObject(new C())->sayHello();