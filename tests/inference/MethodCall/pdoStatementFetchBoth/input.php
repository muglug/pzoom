<?php
/** @return array<null|scalar>|false */
function fetch_both() {
    $p = new PDO("sqlite::memory:");
    $sth = $p->prepare("SELECT 1");
    $sth->execute();
    return $sth->fetch(PDO::FETCH_BOTH);
}
