var current = undefined;
var viewing = undefined;
var invocations = {};

let Host = class {
  constructor(record) {
    this.id = record.id;
    this.hostname = record.hostname;
    this.state = record.state.desc;
    this.target = record.state.target;
    this.listing = undefined;
  }

  makeId() {
    var element = document.createElement("div");
    element.classList.add("id");
    element.appendChild(document.createTextNode(this.id));
    return element;
  }

  makeName() {
    var element = document.createElement("div");
    element.classList.add("hostname");
    element.appendChild(document.createTextNode(this.hostname));
    return element;
  }

  makeState() {
    var element = document.createElement("a");
    element.classList.add("state");
    element.classList.add(this.state);
    element.appendChild(document.createTextNode(this.state));
    // TODO listener
    return element;
  }

  get element() {
    if (this.listing === undefined) {
      this.listing = document.createElement("div");
      this.listing.classList.add("host");
      this.listing.appendChild(this.makeId());
      this.listing.appendChild(this.makeName());
      this.listing.appendChild(this.makeState());
      this.listing.setAttribute("uuid", this.id);
    }
    return this.listing;
  }
};

let Invocation = class {
  constructor(record) {
    this.id = record.id;
    this.name = record.name;
    this.url = record.url;
    this.commit = record.commit;
    this.start = record.start;
    this.listing = undefined;
    if (this.name === null) {
      this.failed = true;
    } else {
      this.failed = false;
    }
  }

  makeId() {
    var element = document.createElement("div");
    element.classList.add("id");
    element.appendChild(document.createTextNode(this.id));
    return element;
  }

  makeName() {
    var element = document.createElement("div");
    element.classList.add("name");
    if (this.failed) {
      element.classList.add("unresolved");
      element.appendChild(document.createTextNode("(failed)")); 
    } else {
      element.appendChild(document.createTextNode(this.name));
    }
    return element;
  }

  makeUrl() {
    var element = document.createElement("a");
    element.classList.add("url");
    element.setAttribute("href", this.url);
    element.appendChild(document.createTextNode(this.url));
    return element;
  }

  makeCommit() {
    var element = document.createElement("div");
    element.classList.add("commit");
    element.appendChild(document.createTextNode(this.commit.substring(0, 10)));
    return element;
  }

  makeExpandButton() {
    var element = document.createElement("a");
    element.classList.add("popout");
    element.classList.add("button");
    element.appendChild(materialIcon("open_in_new"));
    // TODO listener
    return element;
  }

  makeTime() {
    var element = document.createElement("div");
    element.classList.add("time");
    element.appendChild(document.createTextNode(formatDate(new Date(this.start))));
    return element;
  }

  makeStatus() {
    var element;
    if (this.failed) {
      element = materialIcon("clear");
    } else {
      element = materialIcon("check");
      element.classList.add("ok");
    }
    element.classList.add("status");
    return element;
  }

  get element() {
    if (this.listing === undefined) {
      this.listing = document.createElement("div");
      this.listing.classList.add("invocation");
      this.listing.appendChild(this.makeId());
      this.listing.appendChild(this.makeName());
      this.listing.appendChild(this.makeUrl());
      this.listing.appendChild(this.makeCommit());
      this.listing.appendChild(this.makeExpandButton());
      this.listing.appendChild(this.makeTime());
      this.listing.appendChild(this.makeStatus());
      this.listing.setAttribute("uuid", this.id);
    }
    return this.listing;
  }
};

function get(url, callback, err) {
  var xhttp = new XMLHttpRequest();
  xhttp.open("GET", url);
  xhttp.send();
  xhttp.onreadystatechange = (e) => {
    var response;
    try {
      response = JSON.parse(xhttp.responseText);
    } catch (e) { return; }
    if (response.status == "ok") {
      delete response.status;
      callback(response);
    } else {
      if (!('msg' in response)) {
        response.msg = "an error occured";
      }
      err(response.msg);
    }
  }
}

function pad(string) {
  string = "" + string;
  if (string.length == 1) {
    return "0" + string;
  } else {
    return string;
  }
}

function formatDate(date) {
  return pad(date.getHours()) + ":" +
      pad(date.getMinutes()) + ":" +
      pad(date.getSeconds()) + " " +
      date.getDate() + "/" +
      (date.getMonth() + 1) + "/" +
      date.getFullYear();
}

function materialIcon(name) {
  var element = document.createElement("i");
  element.classList.add("material-icons");
  element.appendChild(document.createTextNode(name));
  return element;
}

function makeEmpty() {
  var element = document.createElement("div");
  element.classList.add("invocation");
  var p = document.createElement("p");
  p.classList.add("placeholder");
  p.appendChild(document.createTextNode("no active invocation"));
  element.appendChild(p);
  return element;
}

function updateCurrent() {
  var active = document.getElementById("active");
  get("/api/current", function(response) {
    if (current !== response.id) {
      current = response.id;
      updateInvocations(function() {
        while (active.firstChild) {
          active.removeChild(active.firstChild);
        }
        active.appendChild(invocations[current].element);
      });
    }
  }, function(err) {
    if (current !== undefined) {
      current = undefined;
      updateInvocations(function() {
        while (active.firstChild) {
          active.removeChild(active.firstChild);
        }
        active.appendChild(makeEmpty());
      });
    }
  });
}

function updateInvocations(callback) {
  get("/api/invocations", function(response) {
    for (record of response.invocations) {
      if (!(record.id in invocations)) {
        invocations[record.id] = new Invocation(record);
      }
    }
    for (id in invocations) {
      var index = response.invocations.findIndex(function(record) {
        return record.id == id;
      });
      if (index === -1) {
        delete invocations[id];
      }
    }
    var list = document.getElementById("invocations");
    var children = [];
    if (current !== undefined) {
      children.push(current);
    }
    for (child of list.children) {
      var id = child.getAttribute("uuid");
      children.push(id);
      if (id === current || !(id in invocations)) {
        list.removeChild(child);
      }
    }
    var placeholder = document.getElementById("history_placeholder");
    if (list.children.length == 0) {
      placeholder.classList.remove("hidden");
    } else {
      placeholder.classList.add("hidden");
    }
    for (id in invocations) {
      if (!children.includes(id)) {
        list.appendChild(invocations[id].element);
      }
    }
    callback();
  }, function(err) {});
}

function updateHosts() {
  get("/api/hosts", function(response) {
    var list = document.getElementById("hosts");
    while (list.firstChild) {
      list.removeChild(list.firstChild);
    }
    for (record of response.hosts) {
      list.appendChild((new Host(record)).element);
    }
    var placeholder = document.getElementById("hosts_placeholder");
    if (list.children.length == 0) {
      placeholder.classList.remove("hidden");
    } else {
      placeholder.classList.add("hidden");
    }
  }, function(err) {});
}

setInterval(updateCurrent, 500);
setInterval(updateHosts, 500);

document.addEventListener('DOMContentLoaded', function() {
  updateCurrent();
  updateHosts();
}, false);
