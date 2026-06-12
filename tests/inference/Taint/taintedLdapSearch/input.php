<?php
$ds = ldap_connect('example.com');
$dn = 'o=Psalm, c=US';
$filter = $_GET['filter'];
ldap_search($ds, $dn, $filter, []);
