<?php
/** @return list<array<null|scalar>> */
function fetch_both() : array {
    $p = new PDO("sqlite::memory:");
    $sth = $p->prepare("SELECT 1");
    $sth->execute();
    return $sth->fetchAll(PDO::FETCH_BOTH);
}
