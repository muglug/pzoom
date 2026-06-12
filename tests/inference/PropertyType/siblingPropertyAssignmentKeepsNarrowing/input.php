<?php
class Info { public ?string $id = null; public bool $flag = false; }
function f(Info $info, string $s): void {
    $info->id = $s;
    $info->flag = true;
    echo strtolower($info->id);
}
