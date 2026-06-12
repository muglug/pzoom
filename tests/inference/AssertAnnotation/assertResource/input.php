<?php
/**
 * @param  mixed $foo
 * @psalm-assert resource $foo
 */
function assertResource($foo) : void {
    if (!is_resource($foo)) {
        throw new \Exception("bad");
    }
}
/**
 * @param mixed $value
 *
 * @return resource
 */
function consume($value)
{
    assertResource($value);

    return $value;
}
