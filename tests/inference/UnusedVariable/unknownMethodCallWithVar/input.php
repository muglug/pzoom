<?php
function passesByRef(object $a): void {
    /** @psalm-suppress PossiblyUndefinedVariable */
    $a->passedByRef($b);
}
