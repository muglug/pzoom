<?php
function returnsInt(): int {
    return 1;
}

$obj = (object)returnsInt();
