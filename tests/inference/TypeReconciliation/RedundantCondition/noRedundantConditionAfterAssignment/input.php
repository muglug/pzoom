<?php
/** @param int $i */
function foo($i): void {
    /** @psalm-suppress RedundantConditionGivenDocblockType */
    if ($i !== null) {
        /** @psalm-suppress RedundantCastGivenDocblockType */
        $i = (int) $i;

        if ($i) {}
    }
}