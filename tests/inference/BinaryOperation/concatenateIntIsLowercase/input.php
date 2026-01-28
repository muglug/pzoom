<?php
/**
 * @param non-empty-lowercase-string $bar
 * @return non-empty-lowercase-string
 */
function foobar(string $bar): string
{
    return $bar;
}

/** @var lowercase-string */
$foo = "abc";
/** @var int */
$bar = 123;
foobar($foo . $bar);
