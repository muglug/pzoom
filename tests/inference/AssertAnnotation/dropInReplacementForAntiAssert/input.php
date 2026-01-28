<?php
/**
 * @param mixed $foo
 * @psalm-assert falsy $foo
 */
function abort_if($foo): void
{
    if ($foo) {
        throw new \RuntimeException();
    }
}

/**
 * @param string|null $foo
 */
function removeNullable($foo): string
{
    abort_if(is_null($foo));
    return $foo;
}
