<?php
/** @return list<array<string,null|scalar>> */
function fetch_assoc() : array {
    $p = new PDO("sqlite::memory:");
    $sth = $p->prepare("SELECT 1");
    $sth->execute();
    return $sth->fetchAll(PDO::FETCH_ASSOC);
}
