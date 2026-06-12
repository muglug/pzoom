<?php
function alwaysTrue(): true {
    return true;
}

function alwaysFalse(): false {
    return false;
}

function alwaysNull(): null {
    return null;
}
$true = alwaysTrue();
$false = alwaysFalse();
$null = alwaysNull();
