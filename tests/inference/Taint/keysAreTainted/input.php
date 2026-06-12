<?php
function takesArray(array $arr): void {
    foreach ($arr as $key => $_) {
        echo $key;
    }
}

takesArray([$_GET["bad"] => "good"]);
