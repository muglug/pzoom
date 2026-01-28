<?php
function md5_and_reverse(string $string) : string {
    return strrev(md5($string));
}

$db = new PDO("sqlite:sqlitedb");
$db->sqliteCreateFunction("md5rev", "md5_and_reverse", 1);
