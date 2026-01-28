<?php
/**
 * @param ?string $str
 * @psalm-suppress PossiblyNullArgument
 */
function nullable_ref_modifier(&$str): void {
    if (strlen($str) > 5) {
        $str = null;
    }
}
