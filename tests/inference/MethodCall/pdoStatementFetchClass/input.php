<?php
/** @return object|false */
function fetch_class() {
    $p = new PDO("sqlite::memory:");
    $sth = $p->prepare("SELECT 1");
    $sth->execute();
    return $sth->fetch(PDO::FETCH_CLASS);
}
