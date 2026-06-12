<?php
function foo(?array $arr, string $s) : void {
    /**
     * @psalm-suppress PossiblyNullArrayAccess
     * @psalm-suppress MixedArrayAccess
     */
    if ($arr[$s]["b"] !== true) {
        return;
    }

    /**
     * @psalm-suppress MixedArgument
     * @psalm-suppress MixedArrayAccess
     * @psalm-suppress PossiblyNullArrayAccess
     */
    echo $arr[$s]["c"];
}
