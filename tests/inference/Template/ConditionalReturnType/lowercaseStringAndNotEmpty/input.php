<?php
/**
 * @param lowercase-string $string
 * @psalm-return ($string is non-empty-string ? string : int)
 */
function lowercaseIsNonEmpty($string)
{
    if (!$string) {
        return 1;
    }
    return "";
}
/**
 * @param lowercase-string $string
 * @psalm-return ($string is non-empty-lowercase-string ? string : int)
 */
function lowercaseIsNonEmptyLowercase($string)
{
    if (!$string) {
        return 1;
    }
    return "";
}

/** @var lowercase-string $lowercaseString */
$lowercaseString;
/** @var non-empty-lowercase-string $nonEmptyLowercaseString */
$nonEmptyLowercaseString;

$lowercaseString1 = lowercaseIsNonEmpty($lowercaseString);
$lowercaseString2 = lowercaseIsNonEmptyLowercase($lowercaseString);

$nonEmptyLowercaseString1 = lowercaseIsNonEmpty($nonEmptyLowercaseString);
$nonEmptyLowercaseString2 = lowercaseIsNonEmptyLowercase($nonEmptyLowercaseString);
                
