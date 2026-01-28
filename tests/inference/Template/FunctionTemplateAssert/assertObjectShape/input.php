<?php
final class Foo
{
    public const STATUS_OK = "ok";
    public const STATUS_FAIL = "fail";
}

$foo = new stdClass();

/** @psalm-assert object{status: Foo::STATUS_*} $bar */
function assertObjectShape(object $bar): void {
}

assertObjectShape($foo);
$status = $foo->status;
                
