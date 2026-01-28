<?php
$var = "a";
try {
    if (rand(0, 1)) {
        throw new \Exception();
    }
    $var = "b";
} finally {
    if ($var === "a") {
        echo $var;
    }
}
