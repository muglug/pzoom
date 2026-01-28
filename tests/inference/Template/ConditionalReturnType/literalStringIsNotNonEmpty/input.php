<?php
/**
 * @param literal-string $literalString
 * @psalm-return ($literalString is non-empty-string ? string : int)
 */
function getSomething(string $literalString)
{
    if (!$literalString) {
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
                
