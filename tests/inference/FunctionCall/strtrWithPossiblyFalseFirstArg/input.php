<?php
/**
 * @param false|string $str
 * @param array<string, string> $replace_pairs
 * @return string
 */
function strtr_wrapper($str, array $replace_pairs) {
    /** @psalm-suppress PossiblyFalseArgument */
    return strtr($str, $replace_pairs);
}
