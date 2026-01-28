<?php
/** @param mixed $foo */
function foobar($foo): bool
{
    $bar = &$foo;
    return is_string($foo) && $bar === true;
}
                
