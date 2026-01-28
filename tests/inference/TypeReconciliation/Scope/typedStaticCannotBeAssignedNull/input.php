<?php
function a(): void {
    /** @var string */
    static $foo = "foo";

    $foo = null;
}
