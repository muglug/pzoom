<?php
/** @return bool */
function fetch_both() : bool {
    $p = new PDO("sqlite::memory:");
    $sth = $p->prepare("SELECT 1");
    $sth->execute();
    return $sth->fetch(PDO::FETCH_BOUND);
}
