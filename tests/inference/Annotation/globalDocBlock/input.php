<?php
function f(): string {
    /** @var string $a */
    global $a;
    return $a;
}
