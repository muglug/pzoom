<?php
/**
* @param Closure|callable-string $callable
*/
function use_callable($callable) : void
{
}

/**
* @param Closure|string $var
*/
function test($var) : void
{
    if (is_callable($var))
        use_callable($var);
    else
        echo $var;  // $var should be string, instead it's considered to be Closure|string.
}
