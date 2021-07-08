#!/usr/bin/env python

import requests
import re
from bs4 import BeautifulSoup
from typing import List, Tuple


def get_stations(website: str) -> List[Tuple[str, str]]:
    stations = []
    html = requests.get(website).text
    html_soup = BeautifulSoup(html, "lxml")
    station_infos = html_soup.findAll("a", {"class": "btn button stop-btn m-detailed-stop"})
    for station_info in station_infos:
        try:
            stations.append((station_info["data-name"].replace(" ", "_"),
                            re.split("place-|Boat-", station_info["href"])[1]))
        except:
            breakpoint()
    return stations


def main():
    subway_website = "https://www.mbta.com/stops/subway#subway-tab"
    stations = get_stations(subway_website)
    communter_website = "https://www.mbta.com/stops/commuter-rail#commuter-rail-tab"
    stations += get_stations(communter_website)
    ferry_website = "https://www.mbta.com/stops/ferry#ferry-tab"
    stations += get_stations(ferry_website)
    stations.sort()
    breakpoint()
    pass


if __name__ == "__main__":
    main()
