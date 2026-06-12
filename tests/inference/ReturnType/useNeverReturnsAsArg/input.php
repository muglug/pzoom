<?php
namespace Foo;
/**
 * @return never-returns
 */
function foo() : void {
    exit();
}

function bar(string $s) : void {}

bar(foo());
