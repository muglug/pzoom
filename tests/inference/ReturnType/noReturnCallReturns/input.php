<?php
namespace Foo;
/**
 * @return never-returns
 */
function foo() : void {
    exit();
}

/**
 * @return never-returns
 */
function bar() : void {
    foo();
}
