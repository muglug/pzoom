<?php
/** @return array<string,null|scalar>|false */
function fetch_assoc() {
    $p = new PDO("sqlite::memory:");
    $sth = $p->prepare("SELECT 1");
    $sth->execute();
    return $sth->fetch(PDO::FETCH_ASSOC);
}
