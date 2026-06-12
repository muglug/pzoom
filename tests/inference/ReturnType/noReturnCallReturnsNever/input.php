<?php
namespace Foo;
/**
 * @return never
 */
function foo() : void {
    exit();
}

/**
 * @return never
 */
function bar() : void {
    foo();
}
