CREATE DATABASE photon default CHARACTER SET UTF8;
use photon;
CREATE TABLE imgtbl(
    imagename VARCHAR(300) PRIMARY KEY NOT NULL,
    view INT NOT NULL,
    expired BiGINT  UNSIGNED NOT NULL,
    uploadtime BIGINT UNSIGNED NOT NULL
) ENGINE=INNODB;