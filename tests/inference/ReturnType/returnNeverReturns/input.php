<?php
namespace Foo;
/**
 * @return never-returns
 */
function foo() : void {
    exit();
}

function bar() : void {
    return foo();
}
