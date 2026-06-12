<?php
/**
 * @psalm-type _Alias=array{
 *    foo: string,
 *    bar: string,
 *    baz: array{
 *       a: int,
 *    },
 * }
 */
class Foo { }

/**
 * @param array{
 *    foo: string,
 *    bar: string,
 *    baz: array{
 *       a: int,
 *    },
 * } $foo
 */
function foo(array $foo) : int {
    return count($foo);
}

/**
 * @var array{
 *    foo:string,
 *    bar:string,
 *    baz:string,
 * } $_foo
 */
$_foo = ["foo" => "", "bar" => "", "baz" => ""];
