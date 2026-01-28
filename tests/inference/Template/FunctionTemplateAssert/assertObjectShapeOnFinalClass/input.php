<?php
final class Foo
{
}

$foo = new Foo();

/** @psalm-assert object{status: string} $bar */
function assertObjectShape(object $bar): void {
}

assertObjectShape($foo);
$status = $foo->status;
                
