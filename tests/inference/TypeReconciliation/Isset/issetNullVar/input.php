<?php
function four(?string $s) : void {
    if ($s === null) {
        if (isset($s)) {}
    }
}
