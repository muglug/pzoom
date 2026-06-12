<?php
namespace Foo;
/**
 * @psalm-suppress InvalidReturnType
 * @return never
 */
function foo() : void {}

/** @psalm-suppress NoValue */
$a = foo();
print_r($a);
