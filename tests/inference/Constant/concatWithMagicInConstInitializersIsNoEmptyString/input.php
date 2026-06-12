<?php
class C {
    const DIR = __DIR__ . " - dir";
    const FILE = "file:" . __FILE__;
}
$dir = C::DIR;
$file = C::FILE;
