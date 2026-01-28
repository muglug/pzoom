<?php
/** @return list<null|scalar>|false */
function fetch_named() {
    $p = new PDO("sqlite::memory:");
    $sth = $p->prepare("SELECT 1");
    $sth->execute();
    return $sth->fetch(PDO::FETCH_NUM);
}
