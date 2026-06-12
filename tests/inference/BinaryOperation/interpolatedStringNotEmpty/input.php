<?php
/**
 * @psalm-param non-empty-string $i
 */
function func($i): string
{
    return $i;
}

function foo(string $a) : void {
    func("asdasdasd $a");
}
