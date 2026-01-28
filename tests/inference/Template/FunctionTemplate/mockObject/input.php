<?php
class MockObject {}

/**
 * @psalm-template T1 of object
 * @psalm-param    class-string<T1> $originalClassName
 * @psalm-return   MockObject&T1
 */
function foo(string $originalClassName): MockObject {
    return createMock($originalClassName);
}

/**
 * @psalm-suppress InvalidReturnType
 * @psalm-suppress InvalidReturnStatement
 *
 * @psalm-template T2 of object
 * @psalm-param class-string<T2> $originalClassName
 * @psalm-return MockObject&T2
 */
function createMock(string $originalClassName): MockObject {
    return new MockObject;
}