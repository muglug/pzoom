<?php
/**
 * @param literal-string $string
 * @psalm-return ($string is non-empty-literal-string ? string : int)
 */
function getSomething(string $string)
{
    if (!$string) {
        return 1;
    }

    return "";
}

/** @var literal-string $literalString */
$literalString;
$something = getSomething($literalString);
/** @var non-empty-literal-string $nonEmptyliteralString */
$nonEmptyliteralString;
$something2 = getSomething($nonEmptyliteralString);
                
