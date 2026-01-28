<?php
namespace Foo;

class Bar {}

/**
 * @template Bar as DOMNode
 *
 * @param class-string<Bar> $foo
 */
function Foo(string $foo) : string {
    return $foo;
}
