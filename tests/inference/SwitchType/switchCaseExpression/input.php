<?php
switch (true) {
    case preg_match("/(d)ata/", "some data in subject string", $matches):
        return $matches[1];
    default:
        throw new RuntimeException("none found");
}
