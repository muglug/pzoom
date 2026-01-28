<?php
/**
 * @param literal-string|int $literalStringOrInt
 * @psalm-return ($literalStringOrInt is non-empty-string|int ? string : int)
 */
function getSomething($literalStringOrInt)
{
    if (!$literalStringOrInt) {
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
                
