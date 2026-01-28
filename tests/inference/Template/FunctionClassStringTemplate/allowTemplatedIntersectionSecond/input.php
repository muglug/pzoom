<?php
class MockObject
{
    public function checkExpectations() : void
    {
    }
}

/**
 * @psalm-template RequestedType
 * @psalm-param class-string<RequestedType> $className
 * @psalm-return MockObject&RequestedType
 * @psalm-suppress MixedReturnStatement
 */
function mockHelper(string $className)
{
    eval('"there be dragons"');

    return $instance;
}

class A {
    public function foo() : void {}
}

/**
 * @psalm-param class-string $className
 */
function useMock(string $className) : void {
    mockHelper($className)->checkExpectations();
}

/**
 * @psalm-template UnknownType
 * @psalm-param class-string<UnknownType> $className
 */
function useMockTemplated(string $className) : void
{
    mockHelper($className)->checkExpectations();
}

mockHelper(A::class)->foo();
