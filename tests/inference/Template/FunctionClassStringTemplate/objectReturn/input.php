<?php
/**
 * @template T as object
 *
 * @param class-string<T> $foo
 *
 * @return T
 *
 * @psalm-suppress MixedMethodCall
 */
function Foo(string $foo) : object {
  return new $foo;
}

echo Foo(DateTime::class)->format("c");