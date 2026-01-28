<?php
/** @extends Traversable<string, string> */
interface Foo extends Traversable {}

interface Bar extends Foo {}

/**
 * @return array<string, string>
 */
function foobar(Bar $bar): array
{
    return [...$bar];
}
                
