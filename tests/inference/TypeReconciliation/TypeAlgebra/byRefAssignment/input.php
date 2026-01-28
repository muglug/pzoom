<?php
function foo(): void {
    preg_match("/hello/", "hello molly", $matches);

    if (!$matches) {
        return;
    }

    preg_match("/hello/", "hello dolly", $matches);

    if (!$matches) {

    }
}