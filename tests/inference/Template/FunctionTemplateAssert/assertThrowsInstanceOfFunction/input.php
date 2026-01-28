<?php
namespace Foo;

/**
 * @template T of \Throwable
 * @psalm-param class-string<T> $exceptionType
 * @psalm-assert T $outerEx
 */
function assertThrowsInstanceOf(\Throwable $outerEx, string $exceptionType) : void {
    if (!($outerEx instanceof $exceptionType)) {
        throw new \Exception("thrown instance of wrong type");
    }
}