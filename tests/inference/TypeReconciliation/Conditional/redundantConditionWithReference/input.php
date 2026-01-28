<?php
function foobar(string $foo): bool
{
    $bar = &$foo;
    return is_string($foo) && is_string($bar);
}
                
