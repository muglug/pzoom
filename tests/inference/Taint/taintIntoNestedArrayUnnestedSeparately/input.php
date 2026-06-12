<?php
function foo() : void {
    $a = [[(string) $_GET["bad"]]];
    exec($a[0][0]);
}
