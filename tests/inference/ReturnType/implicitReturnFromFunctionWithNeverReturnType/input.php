<?php
function foo(): never
{
    if (rand(0, 1)) {
        exit();
    }
}
