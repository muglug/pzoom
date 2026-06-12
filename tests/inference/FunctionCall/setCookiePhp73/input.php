<?php
setcookie(
    "name",
    "value",
    [
        "path"     => "/",
        "expires"  => 0,
        "httponly" => true,
        "secure"   => true,
        "samesite" => "Lax"
    ]
);
