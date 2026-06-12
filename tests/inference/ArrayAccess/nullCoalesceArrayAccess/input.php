<?php
/** @param ArrayAccess<int, string> $a */
function foo(?ArrayAccess $a) : void {
    echo $a[0] ?? "default";
}
