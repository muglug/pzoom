<?php
namespace Foo;
/**
 * @psalm-suppress InvalidReturnType
 * @return never
 */
function foo() : void {}

/** @psalm-suppress UnusedParam */
function bar(string $s) : void {}

/** @psalm-suppress NoValue */
bar(foo());
echo "hello";
