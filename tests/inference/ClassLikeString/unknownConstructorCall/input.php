<?php
/** @param class-string $s */
function bar(string $s) : void {
    new $s();
}
