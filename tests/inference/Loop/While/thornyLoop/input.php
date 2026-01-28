<?php

function searchCode(string $content, array &$tmp) : void {
    // separer les balises du texte
    $tmp = [];
    $reg = '/(<[^>]+>)|([^<]+)+/isU';

    // pour chaque element trouve :
    $str    = "";
    $offset = 0;
    while (preg_match($reg, $content, $parse, PREG_OFFSET_CAPTURE, $offset)) {
        $str .= "hello";
        unset($parse);
    }
}
