<?php
class A {}

/**
 * @return A
 * @psalm-suppress MismatchingDocblockReturnType
 */
function foo(): B {
    return new A;
}
