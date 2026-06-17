<?php
function test(): void {
    $a = [];
    /** @psalm-suppress RedundantFunctionCall */
    $a = array_values($a);

    /** @psalm-suppress RedundantConditionGivenDocblockType, PossiblyNullArrayOffset */
    if (empty($a)
        || count($a) > 1
        || empty($a[array_key_first($a)])
    ) {
    }
}
