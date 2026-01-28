<?php
/**
 * @param string|int $stringOrInt
 * @psalm-return ($stringOrInt is (non-empty-string)|int ? string : int)
 */
function getSomething($stringOrInt)
{
    if (!$stringOrInt) {
        return 1;
    }
    return "";
}
/** @var string $string */
$string;
$something = getSomething($string);
/** @var non-empty-string $nonEmptyString */
$nonEmptyString;
$something2 = getSomething($nonEmptyString);
                
