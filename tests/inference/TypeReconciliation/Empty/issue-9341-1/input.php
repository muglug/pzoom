<?php
/** @var string */
$GLOBALS['sql_query'] = rand(0,1) ? 'asd' : null;
if(!empty($GLOBALS['sql_query']) && mb_strlen($GLOBALS['sql_query']) > 2)
{
    exit;
}
