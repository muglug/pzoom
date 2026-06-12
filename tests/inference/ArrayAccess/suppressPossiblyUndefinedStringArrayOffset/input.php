<?php
/** @var array{a?:string} */
$entry = ["a"];

["a" => $elt] = $entry;
strlen($elt);
strlen($entry["a"]);
