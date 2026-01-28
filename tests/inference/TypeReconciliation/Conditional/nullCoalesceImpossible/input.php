<?php
function foo(?string $s) : string {
    return ((string) $s) ?? "bar";
}
