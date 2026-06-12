<?php
$userId = $_GET["user_id"];
$query = "delete from users where user_id = " . $userId;
$mysqli = new mysqli("localhost", "my_user", "my_password", "world");
$result = $mysqli->execute_query($query);
