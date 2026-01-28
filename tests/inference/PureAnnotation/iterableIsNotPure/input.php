<?php
namespace Test;

/**
 * @param iterable<string> $pieces
 *
 * @psalm-pure
 */
function foo(iterable $pieces): string
{
    foreach ($pieces as $piece) {
        return $piece;
    }

    return "jello";
}
