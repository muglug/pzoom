<?php
class A {}
class B {}

/**
 * @param B $_bar
 * @psalm-suppress MismatchingDocblockParamType
 */
function fooFoo(A $_bar): void {
}
